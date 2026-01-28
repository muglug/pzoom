<?php
/** @return array|false */
function array_append(array $errors) {
    if ($errors) {
        return $errors;
    }
    if (rand() % 2 > 0) {
        $errors[] = "unlucky";
    }
    if ($errors) {
        return false;
    }
    return $errors;
}