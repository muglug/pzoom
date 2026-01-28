<?php
/**
 * @param  ?string  $fq_const_name
 * @param  string   $const_name
 * @param array<string, bool> $predefined_constants
 *
 * @return string|null
 */
function getGlobalConstType(
    ?string $fq_const_name,
    string $const_name,
    array $predefined_constants
) {
    if ($const_name === "STDERR"
        || $const_name === "STDOUT"
        || $const_name === "STDIN"
    ) {
        return "hello";
    }

    if ($fq_const_name !== null && (isset($predefined_constants[$fq_const_name])
        || isset($predefined_constants[$const_name])
    )) {
        switch ($const_name) {
            case "PHP_MAJOR_VERSION":
            case "PHP_ZTS":
                return "int";

            case "PHP_FLOAT_EPSILON":
            case "PHP_FLOAT_MAX":
            case "PHP_FLOAT_MIN":
                return "float";
        }

        if (isset($predefined_constants[$fq_const_name])) {
            return "mixed";
        }

        return "hello";
    }

    return null;
}
