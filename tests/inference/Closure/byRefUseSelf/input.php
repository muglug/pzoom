<?php
$external = random_int(0, 1);

$v = function (bool $callMe) use (&$v, $external): void {
    echo($external.PHP_EOL);
    if ($callMe) {
        $v(false);
    }
};

$v(true);
