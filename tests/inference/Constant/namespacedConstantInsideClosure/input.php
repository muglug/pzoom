<?php
namespace Foo;

const FOO_BAR = 1;

function foo(): \Closure {
    return function (): int {
        return FOO_BAR;
    };
}

function foo2(): int {
    return FOO_BAR;
}

$a = function (): \Closure {
    return function (): int {
        return FOO_BAR;
    };
};

$b = function (): int {
    return FOO_BAR;
};
