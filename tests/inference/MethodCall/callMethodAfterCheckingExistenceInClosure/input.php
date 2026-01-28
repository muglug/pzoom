<?php
class A {}

function foo(A $a) : void {
    if (method_exists($a, "bar")) {
        (function() use ($a) : void {
            /** @psalm-suppress MixedArgument */
            echo $a->bar();
        })();

    }
}
