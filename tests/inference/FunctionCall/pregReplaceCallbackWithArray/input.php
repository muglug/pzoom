<?php
/**
 * @param string[] $ids
 */
function(array $ids): array {
    return \preg_replace_callback(
        "//",
        fn (array $matches) => $matches[4],
        $ids
    );
};
