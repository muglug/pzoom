<?php
function f(array $config): void {
    if (isset($config['referencedMethod']) && is_iterable($config['referencedMethod'])) {
        /** @var array $referenced_method */
        foreach ($config['referencedMethod'] as $referenced_method) {
            $method_id = $referenced_method['name'] ?? '';
            if (!is_string($method_id)) {
                throw new \Exception('bad');
            }
            echo $method_id;
        }
    }
}

function g(array $row): void {
    $name = $row['name'] ?? '';
    if (!is_string($name)) {
        return;
    }
    echo $name;
}
