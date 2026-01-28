<?php
class A {}

/** @return class-string */
function foo() : string {
    return A::class;
}

/** @param class-string $a */
function bar(string $a) : void {}

bar(rand(0, 1) ? foo() : A::class);
bar(rand(0, 1) ? A::class : foo());
