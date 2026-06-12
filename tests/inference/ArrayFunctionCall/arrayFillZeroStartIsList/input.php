<?php
class UnionF {}

/**
 * @param array<string, UnionF>|null $template_types
 * @return list<UnionF>
 */
function mappedParams(?array $template_types): array {
    return array_fill(0, count($template_types ?? []), new UnionF());
}
