<?php
/**
 * @return null|array
 */
function maybeReturnArray(): ?array {
    return rand(0, 1) ? null : ["key" => 1];
}

["key" => $a] = maybeReturnArray();
