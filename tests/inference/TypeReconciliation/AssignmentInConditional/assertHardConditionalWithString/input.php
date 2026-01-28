<?php
interface Converter {
    function maybeConvert(string $value): ?SomeObject;
}

interface SomeObject {
    function isValid(): bool;
}

function exampleWithOr(Converter $converter, string $value): SomeObject {
    if (($value = $converter->maybeConvert($value)) === null || !$value->isValid()) {
        throw new Exception();
    }

    return $value; // $value is SomeObject here and cannot be a string
}