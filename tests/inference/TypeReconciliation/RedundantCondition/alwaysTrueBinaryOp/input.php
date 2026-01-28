<?php
function foo ($a) : void {
    if (!$a) return;
    $b = $a && rand(0, 1);
}
