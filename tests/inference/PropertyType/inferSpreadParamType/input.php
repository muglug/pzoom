<?php
class Tag {}
class EntityTags {
    private $tags;

    /** @no-named-arguments */
    public function __construct(Tag ...$tags) {
        $this->tags = $tags;
    }
}
