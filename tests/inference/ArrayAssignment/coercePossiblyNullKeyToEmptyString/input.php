<?php
function string_or_null(): ?string {
  return rand(0, 1) !== 0 ? "aaa" : null;
}

/**
 * @return array<string, null>
 */
function foo(): array {
    $array = [];
    /** @psalm-suppress PossiblyNullArrayOffset */
    $array[string_or_null()] = null;
    return $array;
}
