<?php
function foo(object $object) : void {
    if (!method_exists($object, "foo")) {
        return;
    }
    if (!method_exists($object, "bar")) {
        return;
    }
    $object->foo();
    $object->bar();
}
