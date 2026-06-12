<?php
/**
 * @param Closure():int $x
 */
function accept_closure($x) : void {
    $x();
}
accept_closure(
    function (int $x = 5) : int {
        return $x;
    }
);
