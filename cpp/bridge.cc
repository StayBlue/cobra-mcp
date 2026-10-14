// Runs one simplification the way cobra-cli does (see
// vendor/CoBRA/tools/cobra-cli/main.cpp), returning the result as data
// instead of printing it.

#include "cobra-mcp/cpp/bridge.h"
#include "cobra-mcp/src/cobra.rs.h"

#include "ExprParser.h"
#include "cobra/core/Classifier.h"
#include "cobra/core/Expr.h"
#include "cobra/core/ExprUtils.h"
#include "cobra/core/SignatureChecker.h"
#include "cobra/core/Simplifier.h"

#include <cstdint>
#include <exception>
#include <string>
#include <utility>
#include <vector>

namespace cobra_mcp {

    namespace {

        std::vector< uint64_t >
        EvaluateToSignature(const cobra::Expr &ast, uint32_t num_vars, uint32_t bitwidth) {
            const size_t len = size_t{ 1 } << num_vars;
            std::vector< uint64_t > sig(len);
            std::vector< uint64_t > var_values(num_vars);
            for (size_t i = 0; i < len; ++i) {
                for (uint32_t v = 0; v < num_vars; ++v) { var_values[v] = (i >> v) & 1; }
                sig[i] = cobra::EvalExpr(ast, var_values, bitwidth);
            }
            return sig;
        }

        const char *SemanticClassName(cobra::SemanticClass cls) {
            switch (cls) {
                case cobra::SemanticClass::kLinear:
                    return "linear";
                case cobra::SemanticClass::kSemilinear:
                    return "semilinear";
                case cobra::SemanticClass::kPolynomial:
                    return "polynomial";
                case cobra::SemanticClass::kNonPolynomial:
                    return "non-polynomial";
            }
            return "unknown";
        }

        rust::Vec< rust::String > ToRust(const std::vector< std::string > &strings) {
            rust::Vec< rust::String > out;
            out.reserve(strings.size());
            for (const auto &s : strings) { out.push_back(rust::String(s)); }
            return out;
        }

        SimplifyResponse Failure(const std::string &message) {
            SimplifyResponse response{};
            response.status  = Status::Error;
            response.message = rust::String(message);
            return response;
        }

        SimplifyResponse SimplifyImpl(const SimplifyRequest &request) {
            const uint32_t bitwidth = request.bitwidth;

            auto parsed = cobra::ParseToAst(std::string(request.expr), bitwidth);
            if (!parsed.has_value()) { return Failure(parsed.error().message); }
            const auto &vars = parsed.value().vars;
            const auto num_vars = static_cast< uint32_t >(vars.size());

            // The signature vector has 2^num_vars entries, so reject before
            // building it rather than leaving that to Simplify.
            if (num_vars > request.max_vars) {
                return Failure(
                    "expression has " + std::to_string(num_vars) + " variables; max_vars is "
                    + std::to_string(request.max_vars)
                );
            }

            auto folded = cobra::FoldConstantBitwise(std::move(parsed.value().expr), bitwidth);
            auto cls    = cobra::ClassifyStructural(*folded);
            auto sig    = EvaluateToSignature(*folded, num_vars, bitwidth);

            cobra::Options opts{ .bitwidth   = bitwidth,
                                 .max_vars   = request.max_vars,
                                 .spot_check = true,
                                 .evaluator  = cobra::Evaluator::FromExpr(*folded, bitwidth) };

            auto result = cobra::Simplify(sig, vars, folded.get(), opts);
            if (!result.has_value()) { return Failure(result.error().message); }
            auto &outcome = result.value();

            SimplifyResponse response{};
            response.input_vars     = ToRust(vars);
            response.semantic_class = SemanticClassName(cls.semantic);

            switch (outcome.kind) {
                case cobra::SimplifyOutcome::Kind::kError:
                    return Failure(outcome.diag.reason);
                case cobra::SimplifyOutcome::Kind::kUnchangedUnsupported:
                    response.status  = Status::Unsupported;
                    response.expr    = cobra::Render(*outcome.expr, vars, bitwidth);
                    response.message = rust::String(outcome.diag.reason);
                    return response;
                case cobra::SimplifyOutcome::Kind::kSimplified:
                    break;
            }

            // A change-of-basis result is correct on {0,1} inputs by
            // construction; make sure it also holds at full width.
            std::vector< uint32_t > var_map;
            if (outcome.real_vars.size() < vars.size()) {
                var_map = cobra::BuildVarSupport(vars, outcome.real_vars);
            }
            auto full_width =
                cobra::FullWidthCheck(*folded, num_vars, *outcome.expr, var_map, bitwidth);
            if (!full_width.passed) {
                return Failure("CoB result is only correct on {0,1} inputs (polynomial target)");
            }

            response.status       = Status::Simplified;
            response.expr         = cobra::Render(*outcome.expr, outcome.real_vars, bitwidth);
            response.spot_checked = outcome.verified;
            return response;
        }

    } // namespace

    SimplifyResponse simplify(const SimplifyRequest &request) {
        try {
            return SimplifyImpl(request);
        } catch (const std::exception &e) {
            return Failure(std::string("internal error: ") + e.what());
        }
    }

} // namespace cobra_mcp
