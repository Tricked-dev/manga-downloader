service.migration(None)
service.notifications(None)

core.workflow(
    name = "demo",
    origin = git.github_origin(
        url = "../internal",
        fetch = "main",
    ),
    destination = git.github_pr_destination(
        url = "../public.git",
        destination_ref = "main",
        pr_branch = "codesync/demo",
        title = "Sync demo",
        labels = ["automated-sync"],
        draft = True,
    ),
    origin_files = glob(["src/**", "BUILD.bazel", ".copybara/public/**"]),
    destination_files = glob(["third_party/demo/**"]),
    authoring = authoring.overwrite(default = "TrashCan69420 <trashcan69666@gmail.com>"),
    mode = "SQUASH",
    transformations = [
        core.copy(".copybara/public", "", paths = glob(["README.md"])),
        core.remove(paths = glob(["src/internal_only/**"])),
        core.rename(before = "src/old_name.rs", after = "src/new_name.rs"),
        core.replace(
            before = "//internal/tools",
            after = "//third_party/demo/tools",
            paths = glob(["**/BUILD.bazel", "**/*.bzl"]),
        ),
        core.move("", "third_party/demo"),
        metadata.squash_notes(
            prefix = "Public export:\n",
            oldest_first = True,
            show_author = False,
        ),
        metadata.public_commit_messages(),
    ],
)
