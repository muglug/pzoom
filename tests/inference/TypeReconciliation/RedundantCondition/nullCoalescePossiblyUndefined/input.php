<?php
if (rand(0,1)) {
    $options = ["option" => true];
}

/** @psalm-suppress UndefinedGlobalVariable */
$option = $options["option"] ?? false;

if ($option !== false) {}
