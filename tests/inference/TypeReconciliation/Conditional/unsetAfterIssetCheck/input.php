<?php
function checkbox(array $options = []) : void {
    if ($options["a"]) {}

    unset($options["a"], $options["b"]);
}