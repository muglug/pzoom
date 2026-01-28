<?php
class A {
    public string $s = "hey";
}

/**
 * @psalm-ignore-nullable-return
 */
function foo() : ?A {
    return rand(0, 1) ? new A : null;
}

function takesString(string $_s) : void {}

$foo = foo();

if ($foo->s !== null) {}
echo $foo->s ?? "bar";
takesString($foo->s);
