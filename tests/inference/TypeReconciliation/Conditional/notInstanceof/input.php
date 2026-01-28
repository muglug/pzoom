<?php
class A { }

class B extends A { }

$a = new A();

$out = null;

if ($a instanceof B) {
    // do something
}
else {
    $out = $a;
}