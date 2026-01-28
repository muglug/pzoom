<?php
/**
 * @psalm-template RequestedClass of object
 * @psalm-param class-string<RequestedClass> $templated_class_string
 * @psalm-return class-string<RequestedClass>
 */
function castStringToClassString(
    string $templated_class_string,
    string $input_string
): string {
    \assert(\is_a($input_string, $templated_class_string, true));
    return $input_string;
}