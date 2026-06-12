<?php
/**
 * @psalm-assert array{foo: string} $v
 */
function hasFoo(array $v): void {}

/**
 * @psalm-assert array{bar: int} $v
 */
function hasBar(array $v): void {}

function process(array $data): void {
    hasFoo($data);
    hasBar($data);

    echo sprintf("%s %d", $data["foo"], $data["bar"]);
}
