<?php
/**
 * @param array<string, string> $arr
 */
function foo(array $arr): void {
    if (key_exists("a", $arr)) {
        echo $arr["a"];
    }
}
