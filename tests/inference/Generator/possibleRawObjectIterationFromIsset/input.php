<?php
function foo(array $a) : Generator {
    if (isset($a["a"]["b"])) {
        yield from $a["a"];
    }
}
