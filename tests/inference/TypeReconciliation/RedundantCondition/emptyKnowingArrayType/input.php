<?php
/**
 * @param array<string, array<string, int>> $a
 */
function foo(array $a) : void {
    if (!empty($a["foo"])) {
        foreach ($a["foo"] as $key => $_) {
            if (rand(0, 1)) {
                unset($a["foo"][$key]);
            }
        }
        if (empty($a["foo"])) {}
    }
}