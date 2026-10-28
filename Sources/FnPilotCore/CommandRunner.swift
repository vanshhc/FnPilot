import Foundation

/// Routes one sequence step and executes it against freshly observed targets.
@MainActor
public final class CommandRunner {
    private var count = 0
    public init() {}

    public func interpret(_ part: String, context: CommandContext,
                          fallback: (String, CommandContext) async throws -> Interpretation) async throws -> Interpretation {
        let result: Interpretation
        if context.clarification == nil, let direct = DirectParser().parse(part) ?? DirectUIParser().parse(part, context: context) {
            result = direct
        } else { result = try await fallback(part, context) }
        try Task.checkCancellation()
        try ActionRegistry.validate(result)
        return result
    }

    public func execute(_ result: Interpretation, observed: CommandContext? = nil, context: () -> CommandContext,
                        action: (Action) async throws -> Void) async throws {
        try ActionRegistry.validate(result)
        guard result.status == "actions" else { return }
        for next in result.actions {
            try Task.checkCancellation()
            count += 1
            guard count <= 20 else { throw PilotError("Stopped after 20 actions. Give a shorter instruction.") }
            if let id = next.controlID {
                let current = context()
                guard current.controlsComplete != false, current.controls.contains(where: { $0.id == id }) else {
                    throw PilotError("The selected control is not available in a complete current scan.")
                }
                if let observed {
                    guard let before = observed.controls.first(where: { $0.id == id }),
                          let after = current.controls.first(where: { $0.id == id }),
                          before.role == after.role, before.label == after.label, observed.bundleID == current.bundleID else {
                        throw PilotError("The control changed while interpreting the command.")
                    }
                }
            }
            try await action(next)
            try Task.checkCancellation()
        }
    }
}
