<?php
/**
 * @param mixed $data
 * @return bool
 * @psalm-assert-if-true array{type: string} $data
 */
function isBar($data) {
    return isset($data["type"]);
}

/**
 * @param mixed $data
 * @return string
 */
function doBar($data) {
    if (isBar($data)) {
        return $data["type"];
    }

    throw new \Exception();
}
