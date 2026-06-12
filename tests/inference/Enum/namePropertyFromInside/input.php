<?php
enum Status
{
    case DRAFT;
    case PUBLISHED;
    case ARCHIVED;

    /**
     * @return non-empty-string
     */
    public function get(): string
    {
        return $this->name;
    }
}
