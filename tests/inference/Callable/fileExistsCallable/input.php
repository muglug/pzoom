<?php
/**
 * @param non-empty-string $prospective_file_path
 * @return non-empty-string[]
 */
function foo(string $prospective_file_path) : array {
    return array_filter(
        glob($prospective_file_path),
        "file_exists"
    );
}
