<?php
class A {
    public bool $s = true;
}
function foo(?A $a) : string {
    if (rand(0, 1) && !(isset($a->s) ? $a->s : false)) {
        return "foo";
    }
    return "bar";
}