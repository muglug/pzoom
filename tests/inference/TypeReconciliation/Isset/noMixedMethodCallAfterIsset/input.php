<?php
$data = file_get_contents("php://input");
/** @psalm-suppress MixedAssignment */
$payload = json_decode($data, true);

if (!isset($payload["a"]) || rand(0, 1)) {
    return;
}

/**
 * @psalm-suppress MixedArgument
 */
echo $payload["b"];