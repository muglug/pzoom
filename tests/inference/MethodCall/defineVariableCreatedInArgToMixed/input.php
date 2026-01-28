<?php
function bar($a) : void {
    if ($a->foo($b = (int) "5")) {
        echo $b;
    }
}
