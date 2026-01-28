<?php
class A {}

function foo() : void {
    /**
     * @psalm-suppress UndefinedVariable
     * @psalm-suppress MixedArgument
     */
    if (!is_subclass_of($s, A::class)) {}
}
