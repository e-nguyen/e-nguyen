# Contributing

**You are free to fork the project if any of these guidelines are unacceptable.**

## PR Acceptance Guidelines

Projects that use external graphics and sound API's are notoriously fragile under fast, concurrent development.  Bugs are subtle and platform specific.  Maintaining uncompromising capability to isolate changes is critical to being able to cooperate.  The ancillary benefits of conservative and diligent git practices, such as favoring independence among code hunks,  will inevitably vastly outweight the process overhead of what may seem like detours in the PR process.

* All commits must build so that building is a reliable signal and `git bisect` and other tools can be fully leveraged.  PR's with commits that fail to build will be edited, squashed, or more likely rejected until they are cleaned up.
* Purely additive and bugfix PR's can target the latest release semver if they do not modify any tests' API's or working behavior.
* Additive and behavior changing PR's update the API documentation and update the change log.
* Behavior changing PR's target master branch, which should always contain the next release semver.
* Contributions that cannot include license notices must have an entry in the license [catalog](./licensing/CATALOG.md).  The [CC-BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0/) is recommended for geometry and other raw artistic inputs.
* If you are unable to achieve the quality targets on your own or promptly, you consent to your contribution being modified to achieve quality compliance.

## Licensing

This is a strong copyleft project.  Users have a reasonable expectation that modifications of their work, including artistic, will not after modification become proprietary and unavailable for direct improvement by subsequent users.

* Use strong copyleft licenses on all contributed content that cannot bear an LGPL3 header.  Creative Commons Attibution Share Alike 4.0+ [CC-BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0/) is a good default choice.
* All works are contributed with implied permission to be relicensed under future versions of their respective licenses or licenses with **more** durable and comprehensive copyleft behavior such as LGPL3 -> GPL3.
* Documentation contained within this project itself, including this file, other markdowns, complementary API documentation, and guides & tutorials, where copyrightable, is all born licensed Creative Commons Share-Alike 4.0+.

## Contributor Agreement

In crafting your PR's, you may be asked to affirm or re-affirm that your current and future PR's have been entered into this contributor agreement.

1.  You affirm that you have the right to make the contribution in your PR
2.  You accept the rules in this **Contributing** document and wish to apply them to your entire PR
3.  You consent to the distribution of your contribution under the project's licenses, as indicated in file-level license notifications or the intended license catalog where notices are not practical.

You can explicitly indicate your agreement by appending any two consecutive unicode or github emoji or their equivalents to any (usually the first) commit of the PR and then either making a line comment or mention of your selection in the PR notes.
