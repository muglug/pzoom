<?php
function test(array $x, int $i) : void {
    while (isset($x[$i]) && is_array($x[$i])) {
        $i++;
    }
}
