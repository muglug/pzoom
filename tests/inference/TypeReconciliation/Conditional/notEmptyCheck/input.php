<?php
/**
 * @psalm-suppress MixedAssignment
 */
function load(string $objectName, array $config = []) : void {
    if (isset($config["className"])) {
        $name = $objectName;
        $objectName = $config["className"];
    }
    if (!empty($config)) {}
}