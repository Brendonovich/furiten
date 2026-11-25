cask "furiten" do
  version "@VERSION@"
  sha256 "@SHA256@"

  url "@URL@"
  name "Furiten"
  desc "Spaced-repetition flashcards written in Markdown"
  homepage "https://github.com/anomalyco/furiten"

  livecheck do
    url "https://anoma.ly/release/furiten/desktop/stable.json"
    strategy :json do |json|
      json.dig("release", "version")
    end
  end

  auto_updates true
  depends_on arch: :arm64
  depends_on macos: :ventura

  app "Furiten.app"
  binary "#{appdir}/Furiten.app/Contents/MacOS/furiten"

  uninstall quit: "co.anomaly.furiten"

  zap trash: [
    "~/Library/Application Support/Furiten",
    "~/Library/Caches/co.anomaly.furiten",
    "~/Library/HTTPStorages/co.anomaly.furiten",
    "~/Library/Preferences/co.anomaly.furiten.plist",
    "~/Library/Saved Application State/co.anomaly.furiten.savedState",
  ]
end
