<?php
class TypeUnion {
    public function hasMixed(): bool { return false; }
}

/**
 * @param non-empty-array<string, non-empty-array<string, TypeUnion>> $template_types
 */
function anyNonMixed(array $template_types): bool {
    return array_any(
        $template_types,
        static fn($type_map): bool => !reset($type_map)->hasMixed(),
    );
}
