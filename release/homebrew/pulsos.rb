class Pulsos < Formula
  desc "Cross-platform deployment monitoring CLI"
  homepage "https://github.com/Vivallo04/pulsos-cli"
  version "{{VERSION}}"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/Vivallo04/pulsos-cli/releases/download/v{{VERSION}}/pulsos-aarch64-apple-darwin.tar.gz"
      sha256 "{{SHA256_AARCH64_APPLE_DARWIN}}"
    end

    on_intel do
      url "https://github.com/Vivallo04/pulsos-cli/releases/download/v{{VERSION}}/pulsos-x86_64-apple-darwin.tar.gz"
      sha256 "{{SHA256_X86_64_APPLE_DARWIN}}"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/Vivallo04/pulsos-cli/releases/download/v{{VERSION}}/pulsos-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "{{SHA256_AARCH64_UNKNOWN_LINUX_GNU}}"
    end

    on_intel do
      url "https://github.com/Vivallo04/pulsos-cli/releases/download/v{{VERSION}}/pulsos-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "{{SHA256_X86_64_UNKNOWN_LINUX_GNU}}"
    end
  end

  def install
    bin.install "pulsos"
  end

  test do
    assert_match "pulsos #{version}", shell_output("#{bin}/pulsos --version")
  end
end
