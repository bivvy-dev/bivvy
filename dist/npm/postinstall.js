#!/usr/bin/env node
// Downloads the bivvy binary for the current platform

const { execFileSync } = require("child_process");
const fs = require("fs");
const path = require("path");

const VERSION = require("./package.json").version;
const BIN_DIR = path.join(__dirname, "bin");
const BINARY_NAME = process.platform === "win32" ? "bivvy.exe" : "bivvy";
const BINARY_PATH = path.join(BIN_DIR, BINARY_NAME);
const GITHUB_REPO = "bivvy-dev/bivvy";

function getPlatform() {
  const platformMap = {
    darwin: "darwin",
    linux: "linux",
    win32: "windows",
  };

  const archMap = {
    x64: "x64",
    arm64: "arm64",
  };

  const os = platformMap[process.platform];
  const cpu = archMap[process.arch];

  if (!os || !cpu) {
    throw new Error(`Unsupported platform: ${process.platform}-${process.arch}`);
  }

  return `${os}-${cpu}`;
}

function downloadBinary() {
  const platform = getPlatform();
  const ext = process.platform === "win32" ? "zip" : "tar.gz";
  const url = `https://github.com/${GITHUB_REPO}/releases/download/v${VERSION}/bivvy-${platform}.${ext}`;

  console.log(`Downloading bivvy from ${url}`);

  if (!fs.existsSync(BIN_DIR)) {
    fs.mkdirSync(BIN_DIR, { recursive: true });
  }

  const archivePath = path.join(BIN_DIR, `bivvy.${ext}`);
  execFileSync("curl", ["-fsSL", url, "-o", archivePath]);

  if (ext === "tar.gz") {
    execFileSync("tar", ["-xzf", archivePath, "-C", BIN_DIR]);
    fs.unlinkSync(archivePath);
  }

  if (process.platform !== "win32") {
    fs.chmodSync(BINARY_PATH, 0o755);
  }

  console.log("bivvy installed successfully");
}

if (!fs.existsSync(BINARY_PATH)) {
  downloadBinary();
}
