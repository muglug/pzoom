<?php
class A { }
class B { }
class C { }
$a = rand(0, 10) > 5 ? new A() : new B();
if ($a instanceof A) {
} elseif ($a instanceof C) {
}
