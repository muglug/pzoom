<?php // --taint-analysis

function foo(int $value): void {
    echo $value;
}

/** @psalm-suppress InvalidScalarArgument */
foo($_GET["foo"]);

function bar(): int {
    return $_GET["foo"];
}

echo bar();
