<?php
class A {
    public bool $b = false;
}

function foo(?A $a) : void {
    if (!empty($a->b)) {}
}
