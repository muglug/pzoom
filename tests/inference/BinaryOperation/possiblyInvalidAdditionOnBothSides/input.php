<?php
function foo(string $s) : int {
    return strpos($s, "a") + strpos($s, "b");
}
