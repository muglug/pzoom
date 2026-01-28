<?php
for ($i = 0; $i < 10; $i++) {
    takesInt($i);
}
for (; $i < 20; $i++) {
    takesInt($i);
}

/** @psalm-param int<0, 20> $i */
function takesInt(int $i): void{
    return;
}
