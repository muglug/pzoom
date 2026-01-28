<?php
interface MapperInterface {
    public function map(string $s): int;
}

/**
 * @param list<string> $strings
 * @return list<int>
 */
function mapList(MapperInterface $m, array $strings): array {
    return array_map([$m, "map"], $strings);
}
