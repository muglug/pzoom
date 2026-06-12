<?php
class A {}
class B extends A {}
class C extends A {}

$b = null;

foreach ([new A, new A] as $a) {
    if ($a instanceof B) {

    } elseif (!$a instanceof C) {
        return "goodbye";
    }

    if ($b instanceof C) {
        return "hello";
    }

    $b = $a;
}
