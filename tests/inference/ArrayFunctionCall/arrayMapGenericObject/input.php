<?php
/**
 * @template T
 */
interface Container
{
    /**
     * @return T
     */
    public function get(string $name);
}

/**
 * @param Container<stdClass> $container
 * @param array<string> $data
 * @return array<stdClass>
 */
function bar(Container $container, array $data): array {
    return array_map(
        [$container, "get"],
        $data
    );
}
