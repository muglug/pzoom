<?php
// parameter names in PHP are messed up:
// ArrayObject::offsetSet(mixed $key, mixed $value) : void;
// ArrayAccess::offsetSet(mixed $offset, mixed $value) : void;
// and yet ArrayObject implements ArrayAccess

/** @extends ArrayObject<int, int> */
class C extends ArrayObject {
    public function offsetSet(mixed $key, mixed $value): void {
        parent::offsetSet($key, $value);
    }
}
