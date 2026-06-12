<?php
function getC() : ?C {
    return rand(0, 1) ? new C() : null;
}

function foo() : void {
    echo getC()->id;
}

class C {
    public int $id = 1;
}
