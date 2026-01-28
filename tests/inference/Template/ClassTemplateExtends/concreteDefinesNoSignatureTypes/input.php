<?php
interface IView {}

class ConcreteView implements IView {}

/**
 * @template-covariant TView as IView
 */
interface IViewCreator {
    /** @return TView */
    public function view();
}

/**
 * @template-covariant TView as IView
 * @implements IViewCreator<TView>
 */
abstract class AbstractViewCreator implements IViewCreator {
    public function view() {
        return $this->doView();
    }

    /** @return TView */
    abstract protected function doView();
}

/**
 * @extends AbstractViewCreator<ConcreteView>
 */
class ConcreteViewerCreator extends AbstractViewCreator {
    protected function doView() {
        return new ConcreteView;
    }
}