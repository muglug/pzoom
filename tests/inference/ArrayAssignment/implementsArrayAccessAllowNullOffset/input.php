<?php
/**
 * @template-implements ArrayAccess<?int, string>
 */
class C implements ArrayAccess {
    public function offsetExists(mixed $offset) : bool { return true; }

    public function offsetGet($offset) : string { return "";}

    public function offsetSet(mixed $offset, mixed $value) : void {}

    public function offsetUnset(mixed $offset) : void { }
}

$c = new C();
$c[] = "hello";
