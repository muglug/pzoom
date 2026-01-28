<?php
class A {}
function foo(A $object) : void {
    if (method_exists($object, "foo") && method_exists($object, "bar")) {
        $object->foo();
        $object->bar();
    }
}
