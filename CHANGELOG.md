# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project (will try to) adhere to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

### Legend

The following icons are used to distinguish breaking changes from non-breaking changes.

- 🔥: Breaking change (high impact: will require code changes for most users)
- 💔: Breaking change (low impact: won't require code changes for most users)

## 0.1.8

- Added `flatten` to convert `SnapshotWithUpdates` into a `Stream` of `Snapshot` when `Updates::Item` is `Snapshot`

## 0.1.7

- Added `into_stream` to convert `SnapshotWithUpdates` into a `Stream` of `SnapshotOrUpdate`

## 0.1.6

### Added

- Added `from_stream` constructor for `SnapshotWithUpdates`. This takes the first item of the stream as the 'snapshot' and subsequent items as 'updates'.

## 0.1.5

### Added

- Added `From` implementation for `tokio::sync::watch::Receiver` for `SnapshotWithUpdates`

## 0.1.4

### Added

- Added `map_keys` method to `SnapshotWithUpdates`

## 0.1.3

### Added

- Added `filter` method to `SnapshotWithUpdates`

## 0.1.2

### Added

- Added `filter_keys` method to `SnapshotWithUpdates`

## 0.1.1

### Added

- Added `map_values` method to `SnapshotWithUpdates`

## 0.1.0

Initial release.
