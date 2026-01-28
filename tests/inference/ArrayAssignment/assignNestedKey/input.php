<?php
/**
 * @psalm-suppress MixedAssignment
 * @psalm-suppress MixedArrayOffset
 *
 * @psalm-return array<true>
 */
function getAutoComplete(array $data): array {
    $response = ["s" => []];

    foreach ($data as $suggestion) {
        $response["s"][$suggestion] = true;
    }

    return $response["s"];
}
