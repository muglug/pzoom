<?php
class A {}

function foo(A $a) : void {
    if (method_exists($a, "bar")) {
        /** @psalm-suppress MixedArgument */
        echo $a->bar();
    }
}
