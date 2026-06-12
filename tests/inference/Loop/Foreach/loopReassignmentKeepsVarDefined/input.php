<?php
class Atomic { public string $value = "A"; }

function collect(string $method_name, bool $g, bool $h): void {
    $fq_class_name = "Foo";
    /** @var list<Atomic> $atomics */
    $atomics = [];

    if ($g) {
        echo $fq_class_name;
    } elseif ($h) {
        foreach ($atomics as $atomic_type) {
            if ($fq_class_name === $atomic_type->value) {
                $alt = null;
            } else {
                $fq_class_name = $atomic_type->value;
                $alt = $fq_class_name . "::" . $method_name;
            }

            if (rand(0, 1) === 1 && $alt !== null) {
                break;
            }
        }

        echo $fq_class_name;
    }
}
