<?php
$a = array_map(
    function(int $type, string ...$args):string {
        return "hello";
    },
    [1, 2, 3]
);
