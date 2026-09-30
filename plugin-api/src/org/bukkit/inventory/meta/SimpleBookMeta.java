package org.bukkit.inventory.meta;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import net.kyori.adventure.text.Component;
import net.kyori.adventure.text.TextComponent;

/** The mutable book metadata stored by Foton's API-side ItemStack.
 *
 * <p>Pages are components, as vanilla's written_book_content holds them, so
 * colour and formatting reach the reader. Title and author are the plain
 * strings vanilla stores; a component set through {@link #title(Component)}
 * is written as section-sign text, as Paper writes it.</p>
 */
public final class SimpleBookMeta extends SimpleItemMeta implements WritableBookMeta {
    private static final int MAX_TITLE_LENGTH = 32;

    private String title;
    private String author;
    private List<Component> pages = new ArrayList<>();
    private Generation generation;

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
    }

    @Override
    public List<String> getPages() {
        return pages.stream().map(SimpleBookMeta::text).toList();
    }

    @Override
    public void setPages(List<String> value) {
        List<Component> answer = new ArrayList<>();
        if (value != null) for (String page : value) answer.add(Component.text(page == null ? "" : page));
        pages = answer;
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
    public void setGeneration(Generation value) {
        generation = value;
    }

    @Override
    public Component title() {
        return title == null ? null : Component.text(title);
    }

    @Override
    public SimpleBookMeta title(Component value) {
        setTitle(value == null ? null : foton.ComponentJson.legacy(value));
        return this;
    }

    @Override
    public Component author() {
        return author == null ? null : Component.text(author);
    }

    @Override
    public SimpleBookMeta author(Component value) {
        setAuthor(value == null ? null : foton.ComponentJson.legacy(value));
        return this;
    }

    @Override
    public List<Component> pages() {
        return List.copyOf(pages);
    }

    @Override
    public net.kyori.adventure.inventory.Book pages(List<Component> value) {
        List<Component> answer = new ArrayList<>();
        if (value != null) for (Component page : value) answer.add(page == null ? Component.empty() : page);
        pages = answer;
        return toBuilder().build();
    }

    @Override
    public Component page(int page) {
        return pages.get(index(page));
    }

    @Override
    public void page(int page, Component data) {
        pages.set(index(page), data == null ? Component.empty() : data);
    }

    /** The book as Adventure models one, to hand to {@code Audience#openBook}. */
    public net.kyori.adventure.inventory.Book.Builder toBuilder() {
        net.kyori.adventure.inventory.Book.Builder builder = net.kyori.adventure.inventory.Book.builder().pages(pages);
        if (title != null) builder.title(title());
        if (author != null) builder.author(author());
        return builder;
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
            && generation == book.generation;
    }

    @Override
    public int hashCode() {
        return java.util.Objects.hash(super.hashCode(), title, author, pages, generation);
    }

    /** A page as Bukkit's string API reads it: its text, with section-sign codes for any style. */
    private static String text(Component page) {
        if (page instanceof TextComponent plain && plain.children().isEmpty() && plain.style().isEmpty()) {
            return plain.content();
        }
        return foton.ComponentJson.legacy(page);
    }

    private int index(int page) {
        if (page < 1 || page > pages.size()) {
            throw new IllegalArgumentException(
                "page must be between 1 and " + pages.size() + ", got " + page);
        }
        return page - 1;
    }
}
