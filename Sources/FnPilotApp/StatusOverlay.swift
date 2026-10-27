import SwiftUI

struct StatusOverlay: View {
    @ObservedObject var controller: AppController

    private var needsDetail: Bool {
        ["Could not finish", "Clarification needed", "Setup needed"].contains(controller.phase)
    }

    private var symbol: String {
        switch controller.phase {
        case "Listening": return "mic.fill"
        case "Done": return "checkmark"
        case "Could not finish", "Setup needed": return "exclamationmark.triangle"
        case "Clarification needed": return "questionmark"
        default: return "waveform"
        }
    }

    var body: some View {
        HStack(spacing: 9) {
            Image(systemName: symbol)
                .font(.system(size: 14, weight: .semibold))
                .foregroundStyle(needsDetail ? .orange : .teal)
                .frame(width: 16)
            if needsDetail {
                Text(controller.status)
                    .font(.system(size: 11, weight: .medium))
                    .lineLimit(2)
                    .frame(maxWidth: .infinity, alignment: .leading)
            } else {
                Text(controller.phase)
                    .font(.system(size: 12, weight: .medium))
                    .lineLimit(1)
            }
        }
        .padding(.horizontal, 14)
        .frame(width: needsDetail ? 280 : 200, height: needsDetail ? 64 : 38)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: needsDetail ? 14 : 19))
        .overlay(RoundedRectangle(cornerRadius: needsDetail ? 14 : 19).strokeBorder(.white.opacity(0.12)))
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("\(controller.phase). \(controller.status)")
        .frame(width: 280, height: 64, alignment: .bottom)
    }
}
