<?php
class A {}
class B extends A {}
function getA() : A {
  return new A();
}

$a = getA();
if ($a instanceof B) {
    $a = new B;
}