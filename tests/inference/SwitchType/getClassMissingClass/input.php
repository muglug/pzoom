<?php
class A {}
class B {}

$a = rand(0, 10) ? new A() : new B();

switch (get_class($a)) {
    case C::class:
        break;
}
