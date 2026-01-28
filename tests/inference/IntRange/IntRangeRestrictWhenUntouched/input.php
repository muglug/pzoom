<?php
foreach ([1, 2, 3] as $i) {
    if ($i > 1) {
        takesInt($i);
    }
}

/** @psalm-param int<2, 3> $i */
function takesInt(int $i): void{
    return;
}
