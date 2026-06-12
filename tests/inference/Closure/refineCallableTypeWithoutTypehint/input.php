<?php
/** @param string[][] $arr */
function foo(array $arr) : void {
    array_map(
        function($a) {
            return reset($a);
        },
        $arr
    );
}
