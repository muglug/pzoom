<?php
class A {
    public string $i = "";
}

class B {
    public int $i = 0;
}

/** @param A|B $o */
function takesAorB(object $o) : void {
    echo $o->i;
    if ($o instanceof A) {
        echo strlen($o->i);
    }
}
