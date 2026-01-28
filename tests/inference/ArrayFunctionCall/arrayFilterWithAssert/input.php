<?php
$a = array_filter(
    [1, "hello", 6, "goodbye"],
    function ($s): bool {
        return is_string($s);
    }
);
