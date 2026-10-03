package org.bukkit.inventory.meta;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import net.kyori.adventure.text.Component;
import net.kyori.adventure.text.TextComponent;

/** The mutable book metadata stored by Foton's API-side ItemStack. */
public final class SimpleBookMeta extends SimpleItemMeta implements WritableBookMeta {
    private static final int MAX_TITLE_LENGTH = 32;

    private String title;
    private String author;
    private List<Component> pages = new ArrayList<>();
    private Generation generation = Generation.ORIGINAL;
    /** Original native content, including filtered projections unavailable in Bukkit. */
    private String nativeBookPassthrough;
    /** Native resolution survives ordinary BookMeta edits, independently of filtered content. */
    private boolean nativeBookResolved;

    public String nativeBookPassthrough() {
        return nativeBookPassthrough;
    }

    /** Called after the visible fields have been decoded from a native slot. */
    public void setNativeBookPassthrough(String value) {
        nativeBookPassthrough = value;
    }

    public boolean nativeBookResolved() {
        return nativeBookResolved;
    }

    public void setNativeBookResolved(boolean value) {
        nativeBookResolved = value;
    }

    @Override
    public boolean hasTitle() {
        return title != null;
    }

    @Override
    public String getTitle() {
        return title;
    }

    @Override
    public boolean setTitle(String value) {
        if (value != null && value.length() > MAX_TITLE_LENGTH) {
            return false;
        }
        title = value;
        nativeBookPassthrough = null;
        return true;
    }

    @Override
    public boolean hasAuthor() {
        return author != null;
    }

    @Override
    public String getAuthor() {
        return author;
    }

    @Override
    public void setAuthor(String value) {
        author = value;
        nativeBookPassthrough = null;
    }

    @Override
    public boolean hasPages() {
        return !pages.isEmpty();
    }

    @Override
    public String getPage(int page) {
        return text(pages.get(index(page)));
    }

    @Override
    public void setPage(int page, String data) {
        pages.set(index(page), Component.text(data == null ? "" : data));
        nativeBookPassthrough = null;
    }

    @Override
    public List<String> getPages() {
        return pages.stream().map(SimpleBookMeta::text).toList();
    }

    @Override
    public void setPages(List<String> value) {
        pages = new ArrayList<>();
        if (value != null) for (String page : value) pages.add(Component.text(page == null ? "" : page));
        nativeBookPassthrough = null;
    }

    @Override
    public void setPages(String... value) {
        setPages(value == null ? null : Arrays.asList(value));
    }

    @Override
    public void addPage(String... added) {
        if (added == null) {
            return;
        }
        for (String page : added) {
            pages.add(Component.text(page == null ? "" : page));
        }
        nativeBookPassthrough = null;
    }

    @Override
    public int getPageCount() {
        return pages.size();
    }

    @Override
    public Generation getGeneration() {
        return generation;
    }

    @Override
    public boolean hasGeneration() {
        return generation != Generation.ORIGINAL;
    }

    @Override
    public void setGeneration(Generation value) {
        generation = value == null ? Generation.ORIGINAL : value;
        nativeBookPassthrough = null;
    }

    @Override
    public Component title() {
        return title == null ? null : Component.text(title);
    }

    @Override
    public SimpleBookMeta title(Component value) {
        setTitle(value == null ? null : foton.ComponentJson.plain(value));
        return this;
    }

    @Override
    public Component author() {
        return author == null ? null : Component.text(author);
    }

    @Override
    public SimpleBookMeta author(Component value) {
        setAuthor(value == null ? null : foton.ComponentJson.plain(value));
        return this;
    }

    @Override
    public List<Component> pages() {
        return List.copyOf(pages);
    }

    @Override
    public net.kyori.adventure.inventory.Book pages(List<Component> value) {
        List<Component> copy = new ArrayList<>();
        if (value != null) for (Component page : value) copy.add(page == null ? Component.empty() : page);
        pages = copy;
        nativeBookPassthrough = null;
        return net.kyori.adventure.inventory.Book.book(
            title == null ? Component.empty() : title(),
            author == null ? Component.empty() : author(), pages);
    }

    @Override
    public Component page(int page) {
        return pages.get(index(page));
    }

    @Override
    public void page(int page, Component data) {
        pages.set(index(page), data == null ? Component.empty() : data);
        nativeBookPassthrough = null;
    }

    private static String text(Component page) {
        if (page instanceof TextComponent plain && plain.children().isEmpty() && plain.style().isEmpty()) {
            return plain.content();
        }
        return foton.ComponentJson.plain(page);
    }

    @Override
    public SimpleBookMeta clone() {
        SimpleBookMeta copy = (SimpleBookMeta) super.clone();
        copy.pages = new ArrayList<>(pages);
        return copy;
    }

    @Override
    public boolean equals(Object other) {
        return super.equals(other)
            && other instanceof SimpleBookMeta book
            && java.util.Objects.equals(title, book.title)
            && java.util.Objects.equals(author, book.author)
            && pages.equals(book.pages)
            && generation == book.generation
            && nativeBookResolved == book.nativeBookResolved;
    }

    @Override
    public int hashCode() {
        return java.util.Objects.hash(super.hashCode(), title, author, pages, generation,
            nativeBookResolved);
    }

    private int index(int page) {
        if (page < 1 || page > pages.size()) {
            throw new IllegalArgumentException(
                "page must be between 1 and " + pages.size() + ", got " + page);
        }
        return page - 1;
    }

}
