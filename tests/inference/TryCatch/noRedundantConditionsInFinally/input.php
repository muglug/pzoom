<?php
function doThings(): void {}
function message(): string { return "message"; }

$errors = [];

try {
    doThings();
} catch (RuntimeException $e) {
    $errors["field"] = message();
} catch (LengthException $e) {
    $errors[rand(0,1) ? "field" : "field2"] = message();
} finally {
    if (!empty($errors)) {
        return $errors;
    }
}
