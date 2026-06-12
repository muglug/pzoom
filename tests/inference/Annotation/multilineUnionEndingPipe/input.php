<?php
/**
 * @param 'property'|'psalm-property'|'property-read'|
 *     'psalm-property-read'|'property-write'|'psalm-property-write' $property_tag
 */
function takesTag(string $property_tag): string {
    return $property_tag;
}

function f(): void {
    echo takesTag('property-read');
}
