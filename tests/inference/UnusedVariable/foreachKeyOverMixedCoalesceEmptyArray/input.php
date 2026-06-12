<?php

function takesContents(string $contents): array {
    $composer_json = json_decode($contents, true);
    if (!is_array($composer_json)) {
        throw new UnexpectedValueException('bad json');
    }
    $required_extensions = [];
    /** @psalm-suppress MixedAssignment */
    foreach (($composer_json["require"] ?? []) as $required => $_) {
        if (str_starts_with((string) $required, "ext-")) {
            $required_extensions[strtolower(substr((string) $required, 4))] = true;
        }
    }
    return $required_extensions;
}
