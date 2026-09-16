use super::*;
use crate::nbt::parse_snbt;

#[test]
fn brewing_task13_text_profile_cardinality_precedes_copying() {
    let mut group = NbtCompound::new();
    group.insert("textures", NbtList::String(vec!["value".into(); 128]));
    let mut property = NbtCompound::new();
    property.insert("name", "textures");
    property.insert("value", "value");
    for tag in [
        NbtTag::Compound(group),
        NbtTag::List(NbtList::Compound(vec![property; 128])),
    ] {
        let stats = allocation_counter::measure(|| {
            assert!(parse_properties(Some(&tag)).is_err());
        });
        eprintln!("task13 text profile reject: {stats:?}");
        assert_eq!(
            stats.count_total, 0,
            "profile limit must precede string conversion"
        );
    }
}

#[test]
fn brewing_task13_pinned_text_semantic_parity() {
    let cases = [
        r#""plain""#,
        r#"["first", "second"]"#,
        r"[]",
        r"[1, 2]",
        r#"{text:"x",extra:[{text:"child",bold:true}]}"#,
        r#"{text:"x",extra:[]}"#,
        r#"{text:"x",extra:[{unknown:"x"}]}"#,
        r#"{translate:"key",with:["x",{text:"y"}],fallback:"fallback"}"#,
        r#"{translate:"key",with:[]}"#,
        r#"{text:5,translate:"key",with:["arg"]}"#,
        r#"{translate:"key",with:[3],keybind:"fallback.key"}"#,
        r#"{type:"translatable",translate:"key",with:[3],text:"fallback"}"#,
        r#"{selector:"@p",separator:{text:"sep",extra:["child"]}}"#,
        r#"{selector:"@p",separator:5,text:"preferred"}"#,
        r#"{nbt:"foo",entity:"@p",separator:5}"#,
        r#"{nbt:"foo",entity:"@p",separator:{text:"x"},interpret:true}"#,
        r#"{nbt:"foo",entity:"@p",interpret:true,plain:true}"#,
        r#"{text:"x",hover_event:{action:"show_text",value:{translate:"key",with:["arg"]}}}"#,
        r#"{text:"x",hover_event:{action:"show_entity",id:"minecraft:pig",uuid:[I;0,0,0,1],name:{text:"pig",extra:["child"]}}}"#,
        r#"{text:"x",hover_event:{action:"show_entity",id:"minecraft:pig",uuid:"bad"}}"#,
        r#"{text:"x",hover_event:{action:"show_item",id:"minecraft:stone",count:2,components:{"minecraft:damage":1}}}"#,
        r#"{text:"x",hover_event:{action:"show_item",id:"minecraft:stone",count:0}}"#,
        r#"{sprite:"test:sprite",fallback:{text:"fallback",extra:["child"]}}"#,
        r#"{object:"player",player:{name:"Alex",id:[I;0,0,0,1],model:"slim",properties:{textures:["one","two"]}},fallback:"fallback"}"#,
        r#"{object:"player",player:{name:"bad name"}}"#,
        r#"{object:"player",player:{properties:[{name:"textures",value:"x",signature:"signed"}]}}"#,
        r#"{sprite:"test:sprite",fallback:4}"#,
        r#"{text:"preferred",sprite:"test:sprite",fallback:4}"#,
        r#"{custom:{id:"test:custom",payload:{x:1}}}"#,
        r#"{text:"x",click_event:{action:"custom",id:"test:click",payload:{x:[1,2]}}}"#,
        r#"{text:"x",click_event:{action:"show_dialog",dialog:{type:"test:dialog"}}}"#,
        r#"{text:"x",click_event:{action:"open_url",url:"file:/tmp/foo"}}"#,
        r#"{text:"x",click_event:{action:"change_page",page:0}}"#,
        r#"{text:"x",bold:"bad"}"#,
        r#"{text:"x",color:"unknown"}"#,
        r#"{text:"x",shadow_color:[0.1f,0.2f,0.3f,1.0f]}"#,
        r#"{text:"x",shadow_color:[{}, {}, {}, {}]}"#,
    ];
    for source in cases {
        let tag = parse_snbt(source).expect("fixture SNBT");
        let expected = TextComponent::try_from_nbt(&tag);
        let actual = component(&tag);
        assert_eq!(actual, expected, "{source}");
        if let (Ok(actual), Ok(expected)) = (actual, expected) {
            assert_eq!(actual.to_codec_nbt(), expected.to_codec_nbt(), "{source}");
        }
    }
}

#[test]
fn brewing_task14_pinned_selector_nbt_fallback_precedence() {
    for source in [
        r#"{selector:"s",nbt:"p",separator:{type:"bad"}}"#,
        r#"{selector:"s",nbt:"p",separator:{type:"bad"},entity:"@p"}"#,
        r#"{selector:4,nbt:"p",separator:{text:"sep"},entity:"@p"}"#,
        r#"{selector:"s",nbt:"p",separator:{text:"sep"},entity:"@p"}"#,
        r#"{selector:"s",nbt:"p",separator:{type:"bad"},entity:"@p",interpret:true,plain:true}"#,
        r#"{selector:"s",nbt:"p",separator:{type:"bad"},sprite:"test:sprite"}"#,
        r#"{selector:"s",nbt:"p",separator:{type:"bad"},custom:{id:"test:custom"}}"#,
        r#"{type:"selector",selector:"s",nbt:"p",separator:{type:"bad"},entity:"@p"}"#,
        r#"{type:"nbt",selector:"s",nbt:"p",separator:{type:"bad"},entity:"@p"}"#,
        r#"{type:"nbt",nbt:"p",separator:{text:"sep"}}"#,
        r#"{translate:"key",fallback:{text:"fallback"},with:[{type:"bad"}],nbt:"p",entity:"@p",separator:{type:"bad"}}"#,
    ] {
        let tag = parse_snbt(source).expect("fixture SNBT");
        assert_eq!(
            component(&tag),
            TextComponent::try_from_nbt(&tag),
            "{source}"
        );
    }
}
