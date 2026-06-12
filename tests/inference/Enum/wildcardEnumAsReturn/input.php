<?php
enum E {
    const A = 1;
    case B;
}

/** @return E::* */
function f(): mixed {
    return E::B;
}
