<?php
/**
 * @psalm-taint-escape sleep
 */
function my_escaping_function_for_seconds(mixed $input) : int {};

$seconds = my_escaping_function_for_seconds($_GET["seconds"]);
sleep($seconds);
