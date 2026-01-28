<?php
abstract class Base {}
class A extends Base {}
class AChild extends A {}
class B extends Base {
    public string $s = "";
}

function foo(Base $base): void {
    if (!$base instanceof A || $base instanceof AChild) {
        if ($base instanceof B && rand(0, 1)) {
            echo $base->s;
        }
    }
}
