<?php
class A {}
class B extends A {}
function foo(A $left, A $right) : void {
    if (($left instanceof B && rand(0, 1))
        || ($right instanceof B && rand(0, 1))
    ) {
        if ($left instanceof B
            && rand(0, 1)
            && $right instanceof B
            && rand(0, 1)
        ) {}
    }
}