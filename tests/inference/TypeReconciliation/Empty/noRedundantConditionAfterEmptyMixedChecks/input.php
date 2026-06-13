<?php
function foo($t) : void {
    if (empty($t)) {
        foreach ($GLOBALS["u"] as $a) {
            if (empty($t)) {
                $t = $a;
            }
        }
    }
}
