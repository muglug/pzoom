<?php
$a = array_replace(
    glob(__DIR__ . '/stubs/*.php'),
    glob(__DIR__ . '/stubs/DBAL/*.php'),
);
