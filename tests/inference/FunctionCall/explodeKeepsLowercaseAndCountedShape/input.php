<?php
final class MethodId4 {
    /** @param lowercase-string $method_name */
    public function __construct(public readonly string $fq_class_name, public readonly string $method_name) {}
}

function mk(string $value): MethodId4 {
    $parts = explode('::', strtolower($value));
    return new MethodId4($parts[0], $parts[1]);
}

/** @return list<string> */
function names(string $line): array {
    $names = [];
    $split_by_dollar = explode('$', $line, 2);
    if (count($split_by_dollar) > 1) {
        $split_by_space = explode(' ', $split_by_dollar[1], 2);
        $names[] = $split_by_space[0];
    }
    return $names;
}
